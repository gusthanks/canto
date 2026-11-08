# \EpisodesApi

All URIs are relative to *https://api.spotify.com/v1*

Method | HTTP request | Description
------------- | ------------- | -------------
[**check_users_saved_episodes**](EpisodesApi.md#check_users_saved_episodes) | **GET** /me/episodes/contains | Check User's Saved Episodes 
[**get_a_shows_episodes**](EpisodesApi.md#get_a_shows_episodes) | **GET** /shows/{id}/episodes | Get Show Episodes 
[**get_an_episode**](EpisodesApi.md#get_an_episode) | **GET** /episodes/{id} | Get Episode 
[**get_multiple_episodes**](EpisodesApi.md#get_multiple_episodes) | **GET** /episodes | Get Several Episodes 
[**get_users_saved_episodes**](EpisodesApi.md#get_users_saved_episodes) | **GET** /me/episodes | Get User's Saved Episodes 
[**remove_episodes_user**](EpisodesApi.md#remove_episodes_user) | **DELETE** /me/episodes | Remove User's Saved Episodes 
[**save_episodes_user**](EpisodesApi.md#save_episodes_user) | **PUT** /me/episodes | Save Episodes for Current User 



## check_users_saved_episodes

> Vec<bool> check_users_saved_episodes(ids)
Check User's Saved Episodes 

Check if one or more episodes is already saved in the current Spotify user's 'Your Episodes' library.  **Note:** This endpoint is deprecated. Use [Check User's Saved Items](/documentation/web-api/reference/check-library-contains) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**ids** | **String** |  | [required] |

### Return type

**Vec<bool>**

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_a_shows_episodes

> models::PagingSimplifiedEpisodeObject get_a_shows_episodes(id, market, limit, offset)
Get Show Episodes 

Get Spotify catalog information about an show’s episodes. Optional parameters can be used to limit the number of episodes returned. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**id** | **String** |  | [required] |
**market** | Option<**String**> |  |  |
**limit** | Option<**i32**> |  |  |[default to 20]
**offset** | Option<**i32**> |  |  |[default to 0]

### Return type

[**models::PagingSimplifiedEpisodeObject**](PagingSimplifiedEpisodeObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_an_episode

> models::EpisodeObject get_an_episode(id, market)
Get Episode 

Get Spotify catalog information for a single episode identified by its unique Spotify ID. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**id** | **String** |  | [required] |
**market** | Option<**String**> |  |  |

### Return type

[**models::EpisodeObject**](EpisodeObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_multiple_episodes

> models::GetMultipleEpisodes200Response get_multiple_episodes(ids, market)
Get Several Episodes 

Get Spotify catalog information for several episodes based on their Spotify IDs. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**ids** | **String** |  | [required] |
**market** | Option<**String**> |  |  |

### Return type

[**models::GetMultipleEpisodes200Response**](get_multiple_episodes_200_response.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_users_saved_episodes

> models::PagingSavedEpisodeObject get_users_saved_episodes(market, limit, offset)
Get User's Saved Episodes 

Get a list of the episodes saved in the current Spotify user's library. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**market** | Option<**String**> |  |  |
**limit** | Option<**i32**> |  |  |[default to 20]
**offset** | Option<**i32**> |  |  |[default to 0]

### Return type

[**models::PagingSavedEpisodeObject**](PagingSavedEpisodeObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## remove_episodes_user

> remove_episodes_user(ids, remove_episodes_user_request)
Remove User's Saved Episodes 

Remove one or more episodes from the current user's library.  **Note:** This endpoint is deprecated. Use [Remove Items from Library](/documentation/web-api/reference/remove-library-items) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**ids** | **String** |  | [required] |
**remove_episodes_user_request** | Option<[**RemoveEpisodesUserRequest**](RemoveEpisodesUserRequest.md)> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## save_episodes_user

> save_episodes_user(ids, save_episodes_user_request)
Save Episodes for Current User 

Save one or more episodes to the current user's library.  **Note:** This endpoint is deprecated. Use [Save Items to Library](/documentation/web-api/reference/save-library-items) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**ids** | **String** |  | [required] |
**save_episodes_user_request** | Option<[**SaveEpisodesUserRequest**](SaveEpisodesUserRequest.md)> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)

